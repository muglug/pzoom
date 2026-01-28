<?php
class C {
    public function f(): void {}
}

class C2 extends C {
    #[\Override]
    public function f(): void {}
}
