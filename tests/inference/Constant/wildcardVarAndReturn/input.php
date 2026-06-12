<?php
class Numbers {
    public const ONE = 1;
    public const TWO = 2;
}

class Number {
    /**
     * @var Numbers::*
     */
    private $number;

    /**
     * @param Numbers::* $number
     */
    public function __construct($number) {
        $this->number = $number;
    }

    /**
     * @return Numbers::*
     */
    public function get(): int {
        return $this->number;
    }
}
