<?php
/**
 * @psalm-template TKey of array-key
 * @psalm-template T
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class ArrayCollection
{
    /** @psalm-var array<TKey,T> */
    private $elements;

    /** @psalm-param array<TKey,T> $elements */
    public function __construct(array $elements = [])
    {
        $this->elements = $elements;
    }

    /**
     * @template TNewKey of array-key
     * @template TNew
     * @psalm-param array<TNewKey, TNew> $elements
     * @psalm-return static<TNewKey, TNew>
     */
    protected static function createFrom(array $elements)
    {
        return new static($elements);
    }

    /**
     * @psalm-template U
     * @psalm-param Closure(T=):U $func
     * @psalm-return static<TKey, U>
     */
    public function map(Closure $func)
    {
        $new_elements = array_map($func, $this->elements);
        return self::createFrom($new_elements);
    }
}