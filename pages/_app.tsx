import "../styles/globals.css";
import { AppProps } from "next/app";

const App = ({ Component, pageProps }: AppProps) => {
  return (
    <>
      {/* Context Providers and stuff go here */}
      <Component {...pageProps} />
    </>
  );
};

export default App;
