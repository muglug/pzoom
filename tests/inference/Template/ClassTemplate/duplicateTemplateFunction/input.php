<?php
/**
 * @template T
 */
class Foo
{
    /** @var T */
    private $value;

    /**
     * @template T
     * @param T $value
     * @return self<T>
     */
    static function of($value): self
    {
        return new self($value);
    }

    /**
     * @param T $value
     */
    private function __construct($value)
    {
        $this->value = $value;
    }
}
