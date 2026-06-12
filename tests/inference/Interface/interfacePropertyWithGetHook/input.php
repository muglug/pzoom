<?php
interface I {
    public string $value { get; }
}

class A implements I {
    public string $value {
        get => "hello";
    }
}

function test(I $r): string {
    return $r->value;
}
