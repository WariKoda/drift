"""Explicit-TLS FTP server for drift's manual tests.

The certificate is self-signed and created on first start in FTPS_TLS_DIR,
so drift has to ask whether to trust it. Deleting it makes the next start
present a different certificate.

Environment:
  FTPS_ROOT       directory served to user drift (password secret)
  FTPS_TLS_DIR    where cert.pem and key.pem live
  FTPS_THROTTLE   bytes per second on data connections, 0 = unlimited
  FTPS_MAX_CONS   logins per client IP, 0 = unlimited
"""
import datetime
import ipaddress
import os

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import NameOID
from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import TLS_DTPHandler, TLS_FTPHandler, ThrottledDTPHandler
from pyftpdlib.servers import FTPServer


class Handler(TLS_FTPHandler):
    # RFC 3659 reserves 501 for MLSD on a file. pyftpdlib also sends it for a
    # path that does not exist, where ProFTPD and Pure-FTPd answer 550, the
    # reply drift's missing-file probe relies on.
    def ftp_MLSD(self, path):
        if not self.fs.lexists(path):
            self.respond("550 No such file or directory.")
            return None
        return super().ftp_MLSD(path)


class ThrottledTLSDTPHandler(TLS_DTPHandler, ThrottledDTPHandler):
    pass


def ensure_certificate(directory):
    cert_path = os.path.join(directory, "cert.pem")
    key_path = os.path.join(directory, "key.pem")
    if os.path.exists(cert_path) and os.path.exists(key_path):
        return cert_path, key_path
    key = ec.generate_private_key(ec.SECP256R1())
    now = datetime.datetime.now(datetime.timezone.utc)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "drift manual test")])
    cert = (
        x509.CertificateBuilder()
        .subject_name(name)
        .issuer_name(name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - datetime.timedelta(minutes=5))
        .not_valid_after(now + datetime.timedelta(days=30))
        .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]), critical=False)
        .sign(key, hashes.SHA256())
    )
    with open(key_path, "wb") as f:
        f.write(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    with open(cert_path, "wb") as f:
        f.write(cert.public_bytes(serialization.Encoding.PEM))
    print("new certificate, SHA-256", cert.fingerprint(hashes.SHA256()).hex(":").upper(), flush=True)
    return cert_path, key_path


def main():
    cert_path, key_path = ensure_certificate(os.environ.get("FTPS_TLS_DIR", "/tls"))
    authorizer = DummyAuthorizer()
    authorizer.add_user("drift", "secret", os.environ.get("FTPS_ROOT", "/data"), perm="elradfmwMT")

    handler = Handler
    handler.certfile = cert_path
    handler.keyfile = key_path
    handler.authorizer = authorizer
    handler.tls_control_required = True
    handler.tls_data_required = True
    handler.passive_ports = range(30000, 30010)
    handler.masquerade_address = "127.0.0.1"
    throttle = int(os.environ.get("FTPS_THROTTLE", "0"))
    if throttle:
        ThrottledTLSDTPHandler.read_limit = throttle
        ThrottledTLSDTPHandler.write_limit = throttle
        handler.dtp_handler = ThrottledTLSDTPHandler

    server = FTPServer(("0.0.0.0", 2121), handler)
    server.max_cons_per_ip = int(os.environ.get("FTPS_MAX_CONS", "0"))
    server.serve_forever()


if __name__ == "__main__":
    main()
