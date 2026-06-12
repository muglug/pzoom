<?php
/**
 * @implements Iterator<int, string>
 */
class FooIterator implements \Iterator {
    private ?int $key = null;

    public function current(): string
    {
        return "a";
    }

    public function next(): void
    {
        $this->key = $this->key === null ? 0 : $this->key + 1;
    }

    public function key(): int
    {
        if ($this->key === null) {
            throw new \Exception();
        }
        return $this->key;
    }

    public function valid(): bool
    {
        return $this->key !== null && $this->key <= 3;
    }

    public function rewind(): void
    {
        $this->key = null;
        $this->next();
    }
}

foreach (new FooIterator() as $key => $value) {
    echo $key . " " . $value;
}
