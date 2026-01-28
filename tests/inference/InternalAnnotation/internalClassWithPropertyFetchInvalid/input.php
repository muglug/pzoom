<?php
                    namespace A\B {
                        /**
                         * @internal
                         */
                        class Foo {
                            public int $barBar = 0;
                        }

                        function getFoo(): Foo {
                            return new Foo();
                        }
                    }

                    namespace C {
                        class Bat {
                            public function batBat(): void {
                                \A\B\getFoo()->barBar;
                            }
                        }
                    }
