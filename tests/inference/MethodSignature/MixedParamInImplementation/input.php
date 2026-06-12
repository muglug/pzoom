<?php
interface I
{
    /**
     * @param mixed $a
     */
    public function a($a): void;
}


final class B implements I
{
    public function a(mixed $a): void {}
}
