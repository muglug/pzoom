<?php
trait A {
    public function foo() : string {
        return 5;
    }
}

trait B {
    use A;
}

class C {
    use B;
}
