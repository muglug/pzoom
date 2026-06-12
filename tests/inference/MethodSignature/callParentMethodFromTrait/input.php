<?php
class MyParentClass
{
    /** @return static */
    public function myMethod()
    {
        return $this;
    }
}

trait MyTrait
{
    final public function myMethod() : self
    {
        return parent::myMethod();
    }
}

class MyChildClass extends MyParentClass
{
    use MyTrait;
}
