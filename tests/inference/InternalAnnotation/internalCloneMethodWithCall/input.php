<?php
                    namespace A {
                        class Foo {
                            /**
                             * @internal
                             */
                            public function __clone() {
                            }
                        }
                    }

                    namespace B {
                        class Bat {
                            public function batBat(): void {
                                $a = new \A\Foo;
                                $aa = clone $a;
                            }
                        }
                    }
