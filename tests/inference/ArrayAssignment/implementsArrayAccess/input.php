<?php
/**
 * @implements \ArrayAccess<array-key, mixed>
 */
class A implements \ArrayAccess {
    /**
     * @param  string|int $offset
     * @param  mixed $value
     */
    public function offsetSet($offset, $value): void {}

    /** @param string|int $offset */
    public function offsetExists($offset): bool {
        return true;
    }

    /** @param string|int $offset */
    public function offsetUnset($offset): void {}

    /**
     * @param  string $offset
     * @return mixed
     */
    public function offsetGet($offset) {
        return 1;
    }
}

$a = new A();
$a["bar"] = "cool";
$a["bar"]->foo();
