<?php
class A {
    public function foo(): void {
        if ($this instanceof B) {
            $this->boop();
        }
    }

    private function boop(): void {}
}

class B extends A {
    private function boop(): void {}
}
