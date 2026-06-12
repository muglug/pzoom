<?php
class Base  {
    public function publicMethod() : void {}
}

class Example extends Base {
    public function test() : Closure {
        return Closure::fromCallable([$this, "publicMethod"]);
    }
}
