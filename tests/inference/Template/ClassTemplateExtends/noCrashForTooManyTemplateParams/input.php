<?php
interface InterfaceA {}

class ImplemX implements InterfaceA {}

interface DoStuff {
    public function stuff(InterfaceA $object): void;
}

/**
 * @implements DoStuff<ImplemX>
 */
class DoStuffX implements DoStuff {
    public function stuff(InterfaceA $object): void {}
}

final class Foo {
    /**
     * @template A of InterfaceA
     * @psalm-param DoStuff<A> $stuff
     */
    public function __construct(DoStuff $stuff) {}
}

new Foo(new DoStuffX());
