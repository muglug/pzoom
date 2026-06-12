<?php
interface I {}
enum E implements I { case A; }
function f(I $i): void {
    if ($i === E::A) {
    } else {
    }
}
