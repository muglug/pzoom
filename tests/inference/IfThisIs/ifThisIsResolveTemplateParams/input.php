<?php
/**
 * @template-covariant T
 */
final class Option
{
    /** @return T|null */
    public function unwrap() { throw new RuntimeException("???"); }
}

/**
 * @template-covariant L
 * @template-covariant R
 */
final class Either
{
    /** @return R|null */
    public function unwrap() { throw new RuntimeException("???"); }
}

/**
 * @template T
 */
final class ArrayList
{
    /** @var list<T> */
    private $items;

    /**
     * @param list<T> $items
     */
    public function __construct(array $items)
    {
        $this->items = $items;
    }

    /**
     * @template A
     * @template B
     * @template TOption of Option<A>
     * @template TEither of Either<mixed, B>
     *
     * @psalm-if-this-is ArrayList<TOption|TEither>
     * @return ArrayList<A|B>
     */
    public function compact(): ArrayList
    {
        $values = [];

        foreach ($this->items as $item) {
            $value = $item->unwrap();

            if (null !== $value) {
                $values[] = $value;
            }
        }

        return new self($values);
    }
}

/** @var ArrayList<Either<Exception, int>|Option<int>> $list */
$list = new ArrayList([]);
$numbers = $list->compact();
