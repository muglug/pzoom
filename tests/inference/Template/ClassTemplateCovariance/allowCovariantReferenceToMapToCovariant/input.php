<?php
/** @template-covariant T */
class CovariantReference
{
    /** @var T */
    private $value;

    /** @param T $value */
    public function __construct($value)
    {
        $this->value = $value;
    }

    /** @return T */
    public function get()
    {
        return $this->value;
    }
}

/**
 * @template-covariant T
 */
class C
{
    /** @var CovariantReference<T> */
    private $reference;

    /** @param CovariantReference<T> $reference */
    public function __construct($reference)
    {
        $this->reference = $reference;
    }

    /** @return CovariantReference<T> */
    function getReference()
    {
        return $this->reference;
    }
}