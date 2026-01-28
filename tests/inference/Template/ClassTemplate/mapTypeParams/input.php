<?php
/**
 * @template TKey as array-key
 * @template TValue
 */
class Map {
    /** @var array<TKey, TValue> */
    public $arr;

    /** @param array<TKey, TValue> $arr */
    function __construct(array $arr) {
        $this->arr = $arr;
    }
}

/**
 * @template TInputKey as array-key
 * @template TInputValue
 * @param Map<TInputKey, TInputValue> $map
 * @return Map<TInputKey, TInputValue>
 */
function copyMapUsingProperty(Map $map): Map {
    return new Map($map->arr);
}