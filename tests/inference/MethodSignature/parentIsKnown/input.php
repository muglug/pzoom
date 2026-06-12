<?php
class A {
    public function returnSelf() : self {
        return $this;
    }
}

class B extends A {
    public function returnSelf() : parent {
        return parent::returnSelf();
    }

}
