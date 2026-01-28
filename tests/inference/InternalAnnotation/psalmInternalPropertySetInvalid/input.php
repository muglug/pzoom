<?php
                    namespace A\B {
                        class Foo {
                            /**
                             * @psalm-internal A\B
                             * @var ?int
                             */
                            public $foo;
                        }
                    }
                    namespace A\C {
                        class Bat {
                            public function batBat() : void {
                                $a = new \A\B\Foo;
                                $a->foo = 5;
                            }
                        }
                    }
