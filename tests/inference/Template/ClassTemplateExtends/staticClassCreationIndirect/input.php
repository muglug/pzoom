<?php
/**
 * @template TKey as array-key
 * @template TValue
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class Collection
{
    private $arr;

    /**
     * @param array<TKey, TValue> $arr
     */
    public function __construct(array $arr) {
        $this->arr = $arr;
    }

    /**
     * @template T1 as array-key
     * @template T2
     * @param array<T1, T2> $arr
     * @return static<T1, T2>
     */
    public static function getInstance(array $arr) {
        return new static($arr);
    }

    /**
     * @param array<TKey, TValue> $arr
     * @return static<TKey, TValue>
     */
    public function map(array $arr) {
        return static::getInstance($arr);
    }
}