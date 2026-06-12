<?php
class A1{}
class B1{}

function testFalseable() : A1 {
    /**
     * @psalm-suppress InvalidReturnStatement
     * @psalm-suppress FalsableReturnStatement
     */
    return (rand() % 2 === 0) ? (new B1()) : false;
}
