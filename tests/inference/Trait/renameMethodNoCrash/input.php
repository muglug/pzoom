<?php

trait HelloTrait {
    protected function sayHello() : string {
        return "Hello";
    }
}

class Person {
    use HelloTrait {
        sayHello as originalSayHello;
    }

    protected function sayHello() : string {
        return $this->originalSayHello();
    }
}

class BrokenPerson extends Person {
    protected function originalSayHello() : string {
        return "bad";
    }
}
