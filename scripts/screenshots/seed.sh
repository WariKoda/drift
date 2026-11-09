#!/usr/bin/env bash
# seed.sh STATE DRIFT PORT: creates the demo project "Acme Shop", its remote
# copy and an isolated drift home under STATE. Called by run.sh; it replaces
# STATE/home and STATE/remote.
set -euo pipefail
D=$1 B=$2 PORT=$3
case $D in
/*/.state) ;;
*) echo "seed.sh: STATE must be an absolute path ending in /.state: $D" >&2; exit 2 ;;
esac
[ -d "$D" ] && [ ! -L "$D" ] || { echo "seed.sh: $D is not a directory" >&2; exit 2; }
rm -rf "$D/home" "$D/remote"
H=$D/home
P=$H/work/acme-shop R=$D/remote/acme-shop
mkdir -p "$P" "$R" "$H/work/kunde-a-blog" "$H/work/intranet-portal" "$H/work/landing-pages" "$H/work/old-microsite"

w() { mkdir -p "$(dirname "$1")"; cat > "$1"; }

# ---------- shared files (identical on both sides)
for root in "$P" "$R"; do
w "$root/composer.json" <<'X'
{
    "name": "acme/shop",
    "type": "project",
    "require": {
        "php": ">=8.3",
        "symfony/framework-bundle": "^7.3",
        "twig/twig": "^3.21"
    },
    "autoload": {
        "psr-4": { "Acme\\Shop\\": "src/" }
    }
}
X
w "$root/public/index.php" <<'X'
<?php

use Acme\Shop\Kernel;

require_once dirname(__DIR__).'/vendor/autoload_runtime.php';

return fn (array $context) => new Kernel($context['APP_ENV'], (bool) $context['APP_DEBUG']);
X
w "$root/public/.htaccess" <<'X'
RewriteEngine On
RewriteCond %{REQUEST_FILENAME} !-f
RewriteRule ^ index.php [QSA,L]
X
w "$root/config/services.yaml" <<'X'
services:
    _defaults:
        autowire: true
        autoconfigure: true

    Acme\Shop\:
        resource: '../src/'
X
w "$root/src/Controller/ProductController.php" <<'X'
<?php

namespace Acme\Shop\Controller;

use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;

final class ProductController extends AbstractController
{
    public function detail(string $slug): Response
    {
        return $this->render('product/detail.html.twig', ['slug' => $slug]);
    }
}
X
w "$root/templates/base.html.twig" <<'X'
<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>{% block title %}Acme Shop{% endblock %}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
</head>
<body>
    {% block body %}{% endblock %}
    <script src="/assets/js/cart.js" defer></script>
</body>
</html>
X
done

# ---------- PriceCalculator: changed locally (upload)
w "$R/src/Service/PriceCalculator.php" <<'X'
<?php

namespace Acme\Shop\Service;

use Acme\Shop\Model\Cart;

final class PriceCalculator
{
    private const TAX_RATE = 0.19;

    public function net(Cart $cart): int
    {
        $total = 0;
        foreach ($cart->items() as $item) {
            $total += $item->price * $item->quantity;
        }

        return $total;
    }

    public function gross(Cart $cart): int
    {
        return (int) round($this->net($cart) * (1 + self::TAX_RATE));
    }

    public function shipping(Cart $cart): int
    {
        return $this->net($cart) >= 5000 ? 0 : 490;
    }
}
X
w "$P/src/Service/PriceCalculator.php" <<'X'
<?php

namespace Acme\Shop\Service;

use Acme\Shop\Model\Cart;
use Acme\Shop\Model\Coupon;

final class PriceCalculator
{
    private const TAX_RATE = 0.19;
    private const FREE_SHIPPING_FROM = 4000;

    public function net(Cart $cart, ?Coupon $coupon = null): int
    {
        $total = 0;
        foreach ($cart->items() as $item) {
            $total += $item->price * $item->quantity;
        }

        if ($coupon !== null) {
            $total -= $coupon->discountFor($total);
        }

        return max(0, $total);
    }

    public function gross(Cart $cart, ?Coupon $coupon = null): int
    {
        return (int) round($this->net($cart, $coupon) * (1 + self::TAX_RATE));
    }

    public function shipping(Cart $cart): int
    {
        return $this->net($cart) >= self::FREE_SHIPPING_FROM ? 0 : 490;
    }
}
X

# ---------- CartController: changed locally
w "$R/src/Controller/CartController.php" <<'X'
<?php

namespace Acme\Shop\Controller;

use Acme\Shop\Service\PriceCalculator;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;

final class CartController extends AbstractController
{
    public function __construct(private PriceCalculator $prices) {}

    public function show(): Response
    {
        $cart = $this->cart();

        return $this->render('cart/show.html.twig', [
            'cart' => $cart,
            'total' => $this->prices->gross($cart),
        ]);
    }
}
X
w "$P/src/Controller/CartController.php" <<'X'
<?php

namespace Acme\Shop\Controller;

use Acme\Shop\Service\CouponRepository;
use Acme\Shop\Service\PriceCalculator;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;

final class CartController extends AbstractController
{
    public function __construct(
        private PriceCalculator $prices,
        private CouponRepository $coupons,
    ) {}

    public function show(Request $request): Response
    {
        $cart = $this->cart();
        $coupon = $this->coupons->find($request->query->get('coupon'));

        return $this->render('cart/show.html.twig', [
            'cart' => $cart,
            'coupon' => $coupon,
            'total' => $this->prices->gross($cart, $coupon),
        ]);
    }
}
X

# ---------- app.css: changed remotely (download)
w "$P/public/assets/css/app.css" <<'X'
:root {
    --brand: #2f6fed;
    --text: #1d2433;
    --radius: 6px;
}

body {
    font-family: system-ui, sans-serif;
    color: var(--text);
}

.button {
    background: var(--brand);
    border-radius: var(--radius);
    padding: 0.5rem 1rem;
}
X
w "$R/public/assets/css/app.css" <<'X'
:root {
    --brand: #e4572e;
    --text: #1d2433;
    --radius: 10px;
}

body {
    font-family: system-ui, sans-serif;
    color: var(--text);
}

.button {
    background: var(--brand);
    border-radius: var(--radius);
    padding: 0.6rem 1.2rem;
    font-weight: 600;
}
X

# ---------- cart.js identical
for root in "$P" "$R"; do
w "$root/public/assets/js/cart.js" <<'X'
document.querySelectorAll('[data-add-to-cart]').forEach((button) => {
    button.addEventListener('click', async () => {
        const response = await fetch('/cart/add', {
            method: 'POST',
            body: new URLSearchParams({ product: button.dataset.addToCart }),
        });
        document.querySelector('#cart-count').textContent = (await response.json()).count;
    });
});
X
done

# ---------- local only / remote only
w "$P/src/Service/CouponRepository.php" <<'X'
<?php

namespace Acme\Shop\Service;

use Acme\Shop\Model\Coupon;

final class CouponRepository
{
    /** @param array<string, Coupon> $coupons */
    public function __construct(private array $coupons = []) {}

    public function find(?string $code): ?Coupon
    {
        return $code === null ? null : ($this->coupons[strtoupper($code)] ?? null);
    }
}
X
w "$P/templates/product/detail.html.twig" <<'X'
{% extends 'base.html.twig' %}

{% block body %}
    <h1>{{ product.name }}</h1>
    <button data-add-to-cart="{{ product.id }}" class="button">Add to cart</button>
{% endblock %}
X
mkdir -p "$R/templates/product"; cp "$P/templates/product/detail.html.twig" "$R/templates/product/detail.html.twig"
w "$R/public/maintenance.html" <<'X'
<!doctype html>
<title>Back soon</title>
<p>We are updating the shop. Please try again in a few minutes.</p>
X

# ---------- ignored stuff
printf 'var/\n.env.local\n' > "$P/.gitignore"
mkdir -p "$P/var/cache"; echo cache > "$P/var/cache/routes.php"
echo 'APP_SECRET=demo' > "$P/.env.local"
git -C "$P" init -q

# Modification times decide the suggested directions: the local PHP
# changes are newer (upload), the remote CSS change is newer (download).
find "$P" "$R" -type f -exec touch -t 202611021000 {} +
touch -t 202611091420 "$P/src/Service/PriceCalculator.php" "$P/src/Controller/CartController.php" "$P/src/Service/CouponRepository.php"
touch -t 202611080915 "$R/public/assets/css/app.css"

# ---------- drift home
export HOME=$H XDG_CONFIG_HOME=$H/.config XDG_CACHE_HOME=$H/.cache XDG_STATE_HOME=$H/.local/state
"$B" projects add "Acme Shop" "$P" >/dev/null
"$B" projects add "Kunde A – Blog" "$H/work/kunde-a-blog" >/dev/null
"$B" projects add "Intranet Portal" "$H/work/intranet-portal" >/dev/null
"$B" projects add "Landing Pages" "$H/work/landing-pages" >/dev/null
"$B" projects add "Old Microsite" "$H/work/old-microsite" >/dev/null
"$B" projects archive old-microsite >/dev/null
mkdir -p -m 700 "$H/.config/drift/projects"
cat > "$H/.config/drift/projects/acme-shop.toml" <<X
[[hosts]]
  name = "staging"
  hostname = "127.0.0.1"
  port = $PORT
  user = "drift"
  root_path = "/upload/acme-shop"
  protocol = "sftp"
  [hosts.auth]
    type = "password"
    password = "secret"

[[hosts]]
  name = "production"
  hostname = "shop.example.com"
  port = 22
  user = "deploy"
  root_path = "/var/www/acme-shop"
  protocol = "sftp"
  [hosts.auth]
    type = "agent"
X
chmod 600 "$H/.config/drift/projects/acme-shop.toml"
printf '[ui.diff]\nsyntax = true\n' > "$H/.config/drift/config.toml"
