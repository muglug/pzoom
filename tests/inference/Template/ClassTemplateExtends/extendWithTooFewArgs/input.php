<?php
/**
 * @template TKey of array-key
 * @template T
 * @template-extends IteratorAggregate<TKey, T>
 */
interface Collection extends IteratorAggregate
{
}

/**
 * @psalm-suppress MissingTemplateParam
 * @template T
 * @template TKey of array-key
 * @template-implements Collection<TKey>
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class ArrayCollection implements Collection
{
    /**
     * @psalm-var T[]
     */
    private $elements;

    /**
     * @psalm-param array<T> $elements
     */
    public function __construct(array $elements = [])
    {
        $this->elements = $elements;
    }

    public function getIterator()
    {
        return new ArrayIterator($this->elements);
    }

    /**
     * @psalm-suppress MissingTemplateParam
     *
     * @psalm-param array<T> $elements
     * @psalm-return ArrayCollection<T>
     */
    protected function createFrom(array $elements)
    {
        return new static($elements);
    }
}
