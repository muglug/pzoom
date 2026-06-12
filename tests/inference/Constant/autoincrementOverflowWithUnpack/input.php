<?php
class A {
    public const I = [
        9223372036854775807 => 0,
        ...[1], // this is a fatal error
    ];
}
