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

                    namespace A\B\C {
                        class Bat {
                            public function batBat() : void {
                                echo (new \A\B\Foo)->foo;
                            }
                        }
                    }
