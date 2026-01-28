<?php
/**
 * @psalm-template TKey of array-key
 * @psalm-template T
 * @psalm-consistent-constructor
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
     * @psalm-template U
     * @psalm-param Closure(T=):U $func
     * @psalm-return ArrayCollection<TKey, U>
     */
    public function map(Closure $func)
    {
        $new_elements = array_map($func, $this->elements);
        return Creator::createFrom($new_elements);
    }
}

class Creator {
    /**
     * @template TNewKey of array-key
     * @template TNew
     * @psalm-param array<TNewKey, TNew> $elements
     * @psalm-return ArrayCollection<TNewKey, TNew>
     */
    public static function createFrom(array $elements) {
        return new ArrayCollection($elements);
    }
}