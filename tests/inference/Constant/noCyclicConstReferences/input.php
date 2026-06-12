<?php
class A {
    const FOO = B::FOO;
}

class B {
    const FOO = C::FOO;
}

class C {
    const FOO = A::FOO;
}
