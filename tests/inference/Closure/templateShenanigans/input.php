<?php
class inner {}
class b {
    public inner $key;

    public function __construct() {
        $this->key = new inner;
    }
}

/**
 * @template-covariant TKey as array-key
 * @template TValue as b
 */
class a {
    /**
     * @template TMappedValue
     *
     * @param (\Closure(TValue): TMappedValue)|true $callback Callback or null
     *
     * @return list<$callback is true ? array : TMappedValue>
     */
    public function toArray1(Closure|true $callback): array {
        return [];
    }
    /**
     * @template TMappedValue
     * @template T as (\Closure(TValue): TMappedValue)
     *
     * @param T $callback Callback or null
     *
     * @return list<TMappedValue>
     */
    public function toArray2(Closure $callback): array {
        return [];
    }
    /**
     * @template TMappedValue
     *
     * @param (\Closure(TValue): TMappedValue) $callback Callback or null
     *
     * @return list<TMappedValue>
     */
    public function toArray3(Closure $callback): array {
        return [];
    }
}

$a = (new a)->toArray1(static fn ($obj) => $obj->key);

$b = (new a)->toArray2(static fn ($obj) => $obj->key);

$c = (new a)->toArray3(static fn ($obj) => $obj->key);
