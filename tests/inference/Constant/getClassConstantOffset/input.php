<?php
class C {
    private const A = [ 0 => "string" ];
    private const B = self::A[0];

    public function foo(): string {
        return self::B;
    }
}
