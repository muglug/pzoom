<?php
class A{}

$packages = Collection::fromClassString(A::class);

/**
 * @template T
 */
class Collection{
    /** @var array<T> $items */
    protected $items = [];

    /**
     * @param array<string, T> $items
     */
    public function __construct(array $items = [])
    {
        $this->items = $items;
    }

    /**
     * @template C as object
     * @param class-string<C> $classString
     * @param array<string, C> $elements
     * @return Collection<C>
     */
    public static function fromClassString(string $classString, array $elements = []) : Collection
    {
        return new Collection($elements);
    }
}