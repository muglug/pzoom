<?php
/** @property-read string $foo */
#[\AllowDynamicProperties]
class A {
    public function __construct(string $key, string $value)
    {
        $this->$key = $value;
    }
}

echo (new A("foo", "bar"))->foo;
                    
