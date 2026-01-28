<?php
class Foo {
    public int|null $test = null{
        get {
            return $this->test;
        }
        set(int $value) {
            $this->test = $value;
        }
    }
}
$property_hook = new Foo();
$property_hook->test = 5;
                    
