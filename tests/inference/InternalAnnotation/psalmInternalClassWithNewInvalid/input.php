<?php
                    namespace A\B {
                        /**
                         * @psalm-internal A\B
                         */
                        class Foo { }
                    }

                    namespace A\C {
                        class Bat {
                            public function batBat(): void {
                                $a = new \A\B\Foo();
                            }
                        }
                    }
