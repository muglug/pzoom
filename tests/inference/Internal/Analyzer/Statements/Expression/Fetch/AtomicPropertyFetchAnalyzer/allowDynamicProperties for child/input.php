<?php
/** @property-read string $foo */
#[\AllowDynamicProperties]
class A {
    public function __construct(string $key, string $value)
    {
        $this->$key = $value;
    }
}

class B extends A {}

echo (new B("foo", "bar"))->foo;
                    
