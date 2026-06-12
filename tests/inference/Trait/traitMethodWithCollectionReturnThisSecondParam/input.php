<?php
/**
 * @template T
 * @template V
 */
class Collection {
    /** @var list<V> */
    public array $items = [];
}

trait HasItems {
    /**
     * @return Collection<int, $this>
     */
    public function toCollection(): Collection {
        /** @var Collection<int, $this> */
        return new Collection();
    }
}

class User {
    use HasItems;
}

$a = (new User())->toCollection();
