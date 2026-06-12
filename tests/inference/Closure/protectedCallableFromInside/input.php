<?php
class Base  {
    protected function protectedMethod() : void {}
}

class Example extends Base {
    public function test() : Closure {
        return Closure::fromCallable([$this, "protectedMethod"]);
    }
}
