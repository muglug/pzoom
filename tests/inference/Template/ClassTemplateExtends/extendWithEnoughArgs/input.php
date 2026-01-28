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
 * @template T
 * @template TKey of array-key
 * @template-implements Collection<TKey, T>
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class ArrayCollection implements Collection
{
    /**
     * @psalm-var array<TKey, T>
     */
    private $elements;

    /**
     * @psalm-param array<TKey, T> $elements
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