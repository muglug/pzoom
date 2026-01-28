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

/**
 * @template TValue
 */
trait BridgeTrait
{
    /**
     * @use CollectionTrait<TValue>
     */
    use CollectionTrait;
}

class Service
{
    /**
     * @use BridgeTrait<int>
     */
    use BridgeTrait;

    /**
     * @return array<int>
     */
    public function elements(): array
    {
        return [1, 2, 3, 4];
    }
}