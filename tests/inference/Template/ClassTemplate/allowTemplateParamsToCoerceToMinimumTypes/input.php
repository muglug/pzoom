<?php
/**
 * @psalm-template TKey of array-key
 * @psalm-template T
 */
class ArrayCollection
{
    /**
     * @var array<TKey,T>
     */
    private $elements;

    /**
     * @param array<TKey,T> $elements
     */
    public function __construct(array $elements = [])
    {
        $this->elements = $elements;
    }
}

/** @psalm-suppress MixedArgument */
$c = new ArrayCollection($GLOBALS["a"]);
