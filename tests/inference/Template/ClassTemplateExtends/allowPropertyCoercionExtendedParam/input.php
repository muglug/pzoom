<?php
class Test
{
    /**
     * @var ArrayCollection<int, DateTime>
     */
    private $c;

    public function __construct()
    {
        $this->c = new ArrayCollection();
        $this->c->filter(function (DateTime $dt): bool {
            return $dt === $dt;
        });
    }
}

/**
 * @psalm-template TKey of array-key
 * @psalm-template T
 */
interface Collection
{
    /**
     * @param Closure $p
     *
     * @return Collection A
     *
     * @psalm-param Closure(T=):bool $p
     * @psalm-return Collection<TKey, T>
     */
    public function filter(Closure $p);
}

/**
 * @psalm-template TKey of array-key
 * @psalm-template T
 * @template-implements Collection<TKey,T>
 */
class ArrayCollection implements Collection
{
    /**
     * @psalm-var array<TKey,T>
     * @var array
     */
    private $elements;

    /**
     * @param array $elements
     *
     * @psalm-param array<TKey,T> $elements
     */
    public function __construct(array $elements = [])
    {
        $this->elements = $elements;
    }

    /**
     * {@inheritDoc}
     *
     * @return static
     */
    public function filter(Closure $p)
    {
        return $this;
    }
}