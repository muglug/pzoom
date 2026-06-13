<?php
class B {}
class C {}

class A {
    /** @var ArrayCollection<int, B> */
    public ArrayCollection $b_collection;

    public function __construct() {
        $this->b_collection = new ArrayCollection([]);
        $this->b_collection->add(5, new C());
    }
}

/**
 * @psalm-template TKey
 * @psalm-template T
 */
class ArrayCollection
{
    /**
     * An array containing the entries of this collection.
     *
     * @psalm-var array<TKey,T>
     * @var array
     */
    private $elements = [];

    /**
     * Initializes a new ArrayCollection.
     *
     * @param array $elements
     *
     * @psalm-param array<TKey,T> $elements
     */
    public function __construct(array $elements = [])
    {
        $this->elements = $elements;
    }

    /**
     * @param TKey $key
     * @param T $t
     */
    public function add($key, $t) : void {
        $this->elements[$key] = $t;
    }
}
