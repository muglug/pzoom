<?php
/** @template E as object */
final class Box
{
    /** @var E */
    private $contents;

    /** @param E $contents */
    public function __construct(object $contents)
    {
        $this->contents = $contents;
    }

    /** @param E $thing */
    public function contains(object $thing) : bool
    {
        if ($this->contents !== $thing) {
            return false;
        }

        return true;
    }
}