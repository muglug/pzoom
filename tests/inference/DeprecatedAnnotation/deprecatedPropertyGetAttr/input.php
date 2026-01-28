<?php
class A{
    /**
     * @var ?int
     */
    #[\Deprecated]
    public $foo;
}
echo (new A)->foo;
