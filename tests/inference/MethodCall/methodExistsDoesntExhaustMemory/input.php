<?php
class C {}

function f(C $c): void {
    method_exists($c, 'a') ? $c->a() : [];
    method_exists($c, 'b') ? $c->b() : [];
    method_exists($c, 'c') ? $c->c() : [];
    method_exists($c, 'd') ? $c->d() : [];
    method_exists($c, 'e') ? $c->e() : [];
    method_exists($c, 'f') ? $c->f() : [];
    method_exists($c, 'g') ? $c->g() : [];
    method_exists($c, 'h') ? $c->h() : [];
    method_exists($c, 'i') ? $c->i() : [];
}
