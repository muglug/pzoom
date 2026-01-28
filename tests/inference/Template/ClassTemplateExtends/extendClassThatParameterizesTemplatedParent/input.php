<?php
/**
 * @template T
 */
abstract class Collection
{
    /**
     * @return array<T>
     */
    abstract function elements() : array;

    /**
     * @return T|null
     */
    public function first()
    {
        return $this->elements()[0] ?? null;
    }
}

/**
 * @template-extends Collection<int>
 */
abstract class Bridge extends Collection {}


class Service extends Bridge
{
    /**
     * @return array<int>
     */
    public function elements(): array
    {
        return [1, 2, 3];
    }
}

$a = (new Service)->first();