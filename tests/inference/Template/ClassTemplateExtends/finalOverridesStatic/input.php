<?php
/**
 * @template T
 */
class Collection {
    /**
     * @param T $item
     */
    public function __construct($item) {}
}

/**
 * @psalm-consistent-constructor
 */
abstract class Food {
    /**
     * @return Collection<static>
     */
    public function getTypes() {
        return new Collection(new static);
    }
}

final class Cheese extends Food {}

/**
 * @return Collection<Cheese>
 */
function test(Cheese $cheese): Collection {
    return $cheese->getTypes();
}