<?php
/** @template T */
class Option
{
    /** @param T $v */
    public function __construct(private $v) {}

    /**
     * @template E
     * @param E $else
     * @return T|E
     */
    public function getOrElse($else)
    {
       return rand(0, 1) === 1 ? $this->v : $else;
    }
}

$opt = new Option([1, 3]);

$b = $opt->getOrElse([2, 4])[0];