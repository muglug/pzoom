<?php
/**
 * @property string $test
 */
class C {
    public function __get(string $name)
    {
    }

    /**
     * @param mixed $value
     */
    public function __set(string $name, $value)
    {
    }

    public function test(): string
    {
        return $this->test;
    }
}
