<?php
/**
 * @method string sayHello()
 */
interface A {}

function makeConcrete() : A {
    return new class implements A {
        function sayHello() : string {
            return "Hello";
        }
    };
}

echo makeConcrete()->sayHello();
