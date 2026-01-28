<?php
interface I1 {}
interface I2 {}

class A
{
    public function foo(): void {
        if ($this instanceof I1 || $this instanceof I2) {}
    }
}