<?php
class C {
    public string $a = "zzz";
    public function f(): void {
        $f = static function (): void {
            echo $this->a;
        };
        $f();
    }
}
