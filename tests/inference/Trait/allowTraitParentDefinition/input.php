<?php
class A {}

class C extends A
{
    use T;
}

trait T
{
    public function bar() : ?C
    {
        if ($this instanceof A) {
            return $this;
        }

        return null;
    }
}
