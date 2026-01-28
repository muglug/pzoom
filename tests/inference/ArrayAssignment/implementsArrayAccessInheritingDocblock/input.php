<?php
/**
 * @implements \ArrayAccess<string, mixed>
 */
class A implements \ArrayAccess
{
    /**
     * @var array<string, mixed>
     */
    protected $data = [];

    /**
     * @param array<string, mixed> $data
     */
    public function __construct(array $data = [])
    {
        $this->data = $data;
    }

    /**
     * @param  string $offset
     */
    public function offsetExists($offset): bool
    {
        return isset($this->data[$offset]);
    }

    /**
     * @param  string $offset
     */
    public function offsetGet($offset)
    {
        return $this->data[$offset];
    }

    /**
     * @param  string $offset
     * @param  mixed  $value
     */
    public function offsetSet($offset, $value): void
    {
        $this->data[$offset] = $value;
    }

    /**
     * @param  string $offset
     */
    public function offsetUnset($offset): void
    {
        unset($this->data[$offset]);
    }
}

class B extends A {
    /**
     * {@inheritdoc}
     */
    public function offsetSet($offset, $value): void
    {
        echo "some log";
        $this->data[$offset] = $value;
    }
}
