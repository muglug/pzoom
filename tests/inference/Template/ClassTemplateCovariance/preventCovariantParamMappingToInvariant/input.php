<?php
/** @template T */
class InvariantReference
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
    /** @var InvariantReference<T> */
    private InvariantReference $reference;

    /** @param InvariantReference<T> $reference */
    public function __construct(InvariantReference $reference)
    {
        $this->reference = $reference;
    }

    /** @return InvariantReference<T> */
    public function getReference() : InvariantReference
    {
        return $this->reference;
    }
}
