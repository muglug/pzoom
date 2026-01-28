<?php
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
