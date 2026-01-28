<?php
/**
 * @template T
 */
final class Option
{
    /**
     * @return T|null
     */
    public function unwrap()
    {
        throw new RuntimeException("???");
    }
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
     * @psalm-if-this-is ArrayList<Option<int>>
     * @return ArrayList<int>
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

/** @var ArrayList<Option<int>> $list */
$list = new ArrayList([]);
$numbers = $list->compact();
