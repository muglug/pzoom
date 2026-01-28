<?php
                    namespace A\B {
                        class Foo {
                            /** @psalm-internal A\B */
                            public int $foo = 0;

                            public function barBar() : void {
                                $this->foo = 42;
                            }
                        }
                    }
