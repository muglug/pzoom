<?php
class A {
    /**
     * @var mixed
     */
    private $mixed = "hello";

    /**
     * @param mixed $value
     */
    public function setMixed($value): void
    {
        $this->mixed = $value;
    }
}
