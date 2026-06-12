<?php
class A
{
    public function __construct(
        public string $name,
    ) {}
}

class B extends A
{
    public function __construct() {}
}
