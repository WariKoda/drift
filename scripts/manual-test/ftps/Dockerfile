FROM python:3.13-slim
RUN pip install --no-cache-dir pyftpdlib==2.2.0 pyOpenSSL==26.4.0
COPY server.py /server.py
EXPOSE 2121 30000-30009
CMD ["python", "/server.py"]
