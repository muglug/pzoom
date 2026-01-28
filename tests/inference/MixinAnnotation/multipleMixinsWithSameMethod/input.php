<?php

class Mix1
{
    public function foo(): string
    {
        return "";
    }
}

class Mix2
{
    public function foo(): string
    {
        return "";
    }
}

/**
 * @mixin Mix1
 * @mixin Mix2
 */
class Bar
{

}

$bar = new Bar();

$bar->foo();
