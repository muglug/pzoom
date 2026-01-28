<?php
/**
 * @template T
 */
trait CollectionTrait
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

class Service
{
    /**
     * @use CollectionTrait<int>
     */
    use CollectionTrait;

    /**
     * @return array<int>
     */
    public function elements(): array
    {
        return [1, 2, 3, 4];
    }
}