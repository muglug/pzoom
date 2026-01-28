<?php
class Item {
    private int $i = 0;

    public function mutate(): void {
        $this->i++;
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
    private Item $item;

    public function __construct(Item $item) {
        $this->item = clone $item;
    }

    public function get(): int {
        return $this->item->get();
    }
}

$item = new Item();
new Immutable($item);
