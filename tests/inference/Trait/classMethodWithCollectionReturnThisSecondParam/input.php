<?php
/**
 * @template T
 * @template V
 */
class Collection {
    /** @var list<V> */
    public array $items = [];
}

class User {
    /**
     * @return Collection<int, $this>
     */
    public function toCollection(): Collection {
        /** @var Collection<int, $this> */
        return new Collection();
    }
}

$a = (new User())->toCollection();
