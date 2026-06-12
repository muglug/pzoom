<?php
interface I
{
    public function __destruct();
}

class C implements I
{
    public function __destruct() {}
}
