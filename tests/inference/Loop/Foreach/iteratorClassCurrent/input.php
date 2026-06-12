<?php
class Value {}

/**
 * @psalm-suppress MissingTemplateParam
 */
class ValueCollection implements \Countable, \IteratorAggregate {
    /**
     * @var Value[]
     */
    private $items = [];

    private function add(Value $item): void
    {
        $this->items[] = $item;
    }

    /**
     * @return Value[]
     */
    public function toArray(): array
    {
        return $this->items;
    }

    public function getIterator(): ValueCollectionIterator
    {
        return new ValueCollectionIterator($this);
    }

    public function count(): int
    {
        return \count($this->items);
    }

    public function isEmpty(): bool
    {
        return empty($this->items);
    }

    public function contains(Value $item): bool
    {
        foreach ($this->items as $_item) {
            if ($_item === $item) {
                return true;
            }
        }

        return false;
    }
}

/**
 * @psalm-suppress MissingTemplateParam
 */
class ValueCollectionIterator implements \Countable, \Iterator
{
    /**
     * @var Value[]
     */
    private $items;

    /**
     * @var int
     */
    private $position = 0;

    public function __construct(ValueCollection $collection)
    {
        $this->items = $collection->toArray();
    }

    public function count(): int
    {
        return \iterator_count($this);
    }

    public function rewind(): void
    {
        $this->position = 0;
    }

    public function valid(): bool
    {
        return $this->position < \count($this->items);
    }

    public function key(): int
    {
        return $this->position;
    }

    public function current(): Value
    {
        return $this->items[$this->position];
    }

    public function next(): void
    {
        $this->position++;
    }
}

function foo(ValueCollection $v) : void {
    foreach ($v as $value) {}
}
