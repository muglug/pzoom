<?php
/** @template T */
abstract class Foo {
    /** @psalm-var T */
    protected $value;

    /** @psalm-param T $value */
    public function __construct($value)
    {
        /** @var T */
        $value = $this->normalize($value);
        $this->value = $value;
    }

    /**
     * @psalm-param T $value
     * @psalm-return T
     */
    protected function normalize($value)
    {
        return $value;
    }
}
                
