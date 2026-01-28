<?php
/** @psalm-immutable */
class Cart {
    /** @var CartItem[] */
    public array $items;

    /** @param CartItem[] $items */
    public function __construct(array $items) {
        $this->items = $items;
    }

    public function addItem(CartItem $item) : self {
        $items = $this->items;
        $items[] = $item;
        return new Cart($items);
    }
}

/** @psalm-immutable */
class CartItem {
    public string $name;
    public float $price;

    public function __construct(string $name, float $price) {
        $this->name = $name;
        $this->price = $price;
    }
}

/** @psalm-pure */
function addItemToCart(Cart $c, string $name, float $price) : Cart {
    return $c->addItem(new CartItem($name, $price));
}
