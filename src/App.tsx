import { QueryClientProvider } from "@tanstack/react-query";
import "./App.css";
import "react";
import { BrowserRouter, useRoutes } from "react-router-dom";

import routes from "~react-pages";
import { queryClient } from "./queries/queryClient";

function Routes() {
  return useRoutes(routes);
}

function App() {
  console.log(routes);

  return (
    <BrowserRouter>
      <QueryClientProvider client={queryClient}>
        <Routes />

        {/* Measures dom elements */}
        <div
          id="measure-root"
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            visibility: "hidden",
            pointerEvents: "none",
            zIndex: -1,
          }}
        />
      </QueryClientProvider>
    </BrowserRouter>
  );
}

export default App;
