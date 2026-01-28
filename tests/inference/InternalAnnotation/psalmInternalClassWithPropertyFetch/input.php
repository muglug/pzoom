<?php
                    namespace A\B {
                        /**
                         * @psalm-internal A\B
                         */
                        class Foo {
                            public int $barBar = 0;
                        }

                        function getFoo(): Foo {
                            return new Foo();
                        }
                    }

                    namespace A\B\C {
                        class Bat {
                            public function batBat(\A\B\Foo $instance): void {
                                \A\B\getFoo()->barBar;
                            }
                        }
                    }
