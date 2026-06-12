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
    public function getCollection(): Collection {
        /** @var Collection<int, $this> */
        $c = new Collection();
        return $c;
    }
}

$a = (new User())->getCollection();
