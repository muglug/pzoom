<?php
final class Holder {
    /** @var list<int> */
    private array $items = [];

    /** @psalm-mutation-free */
    public function hasPositive(): bool {
        return array_any($this->items, static fn(int $i): bool => $i > 0);
    }
}
