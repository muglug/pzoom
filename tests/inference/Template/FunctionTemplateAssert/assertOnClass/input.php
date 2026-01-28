<?php
/**
 * @template T
 */
abstract class Type
{
    /**
     * @param mixed $value
     * @return bool
     * @psalm-assert-if-true T $value
     */
    abstract public function matches($value): bool;

    /**
     * @param mixed $value
     * @return mixed
     * @psalm-return T
     * @psalm-assert T $value
     */
    public function assert($value)
    {
        assert($this->matches($value));
        return $value;
    }
}