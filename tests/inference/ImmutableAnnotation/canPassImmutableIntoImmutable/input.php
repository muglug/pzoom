<?php
/**
 * @psalm-immutable
 */
class Item {
    private int $i;

    public function __construct(int $i) {
        $this->i = $i;
    }

    /** @psalm-mutation-free */
    public function get(): int {
        return $this->i;
    }
}

/**
 * @psalm-immutable
 */
class Immutable {
    private $item;

    public function __construct(Item $item) {
        $this->item = $item;
    }

    public function get(): int {
        return $this->item->get();
    }
}

$item = new Item(5);
new Immutable($item);
