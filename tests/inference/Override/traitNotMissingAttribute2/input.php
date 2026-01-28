<?php
trait B {
    public function f(): void {}
}

class C {
    public function f(): void {}
}

class C2 extends C {
    use B;
}
