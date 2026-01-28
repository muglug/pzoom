<?php
trait B {
    public function f(): void {}
}

class C {
    use B;
}

class C2 extends C {
    use B;
}
