<?php
class A {
    /**
     * @var string
     */
    private $mixed;

    /**
     * @param mixed $value
     */
    public function setMixed($value): void
    {
        $this->mixed = $value;
    }
}
